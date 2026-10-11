//! Sealed Crystal successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::crystal_public_v1::CrystalObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::crystal_v22::CrystalDecisionBindingV22;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV22 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV22 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV22 {}
pub struct HumanCrystalSessionV22 {
    inner: FastActorSessionV1,
    authority: AuthorityV22,
}
#[derive(Clone)]
pub struct HumanCrystalSnapshotV22 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanCrystalSessionV22 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_crystal_v22()?,
            authority: AuthorityV22 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v22(&self) -> HumanCrystalSnapshotV22 {
        HumanCrystalSnapshotV22 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v22(&mut self, snapshot: &HumanCrystalSnapshotV22) -> Result<(), Error> {
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
    ) -> Result<crate::crystal_public_v1::CrystalObservationV1, crate::rl::RlContractError> {
        self.inner
            .crystal_current_decision_input_v22(expected, expected.acting_player)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Crystal public decision".into()))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionCrystalV22 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanCrystalExtensionsV22>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanCrystalSourceV22 {
    pub source: HumanCardRefV1,
    pub controller: PlayerSeatV1,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanCrystalOptionV22 {
    pub kind: String,
    pub source: HumanCardRefV1,
    pub shield_remaining: Option<i32>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanCrystalMenuV22 {
    pub chooser: PlayerSeatV1,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_history: Option<crate::crystal_public_v1::SavedDamageSourceV1>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub simultaneous_choices_remaining: Option<usize>,
    pub source: HumanCardRefV1,
    pub recipient: HumanCardRefV1,
    pub remaining_damage: i32,
    pub options: Vec<HumanCrystalOptionV22>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanCrystalExtensionsV22 {
    pub predecessor: HumanExtensionsV9,
    pub static_sources: Vec<HumanCrystalSourceV22>,
    pub prevention_menu: Option<HumanCrystalMenuV22>,
}
fn parts(observation: &CrystalObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &CrystalObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanCrystalExtensionsV22>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let static_sources = observation
        .static_sources
        .iter()
        .map(|row| {
            Ok(HumanCrystalSourceV22 {
                source: handles.get(&row.source)?.clone(),
                controller: row.controller,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let prevention_menu = observation
        .prevention_menu
        .as_ref()
        .map(|row| {
            let options = row
                .options
                .iter()
                .map(|option| {
                    Ok(HumanCrystalOptionV22 {
                        kind: option.kind.clone(),
                        source: handles.get(&option.source)?.clone(),
                        shield_remaining: option.shield_remaining,
                    })
                })
                .collect::<Result<Vec<_>, Error>>()?;
            Ok::<_, Error>(HumanCrystalMenuV22 {
                chooser: row.chooser,
                source_history: row.source_history.clone(),
                simultaneous_choices_remaining: row.simultaneous_choices_remaining,
                source: handles.get(&row.source)?.clone(),
                recipient: handles.get(&row.recipient)?.clone(),
                remaining_damage: row.remaining_damage,
                options,
            })
        })
        .transpose()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanCrystalExtensionsV22 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            static_sources,
            prevention_menu,
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
    observation: &CrystalObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let Some(menu) = observation.prevention_menu.as_ref() {
        let ActionSemanticV1::ChooseEffectOption {
            actor,
            source,
            option_index,
            option_count,
        } = action
        else {
            return Err(Error::UnsupportedPrompt);
        };
        if *actor != human
            || menu.chooser != human
            || *source != menu.source
            || usize::from(*option_count) != menu.options.len()
        {
            return Err(Error::InvalidVisibleReference);
        }
        let option = menu
            .options
            .get(usize::from(*option_index))
            .ok_or(Error::InvalidAction)?;
        let name = handles.name(&option.source)?;
        let target = handles.name(&menu.recipient)?;
        return Ok(match option.kind.as_str() {
            "barricade" => {
                format!("Apply {name}: prevent all remaining noncombat damage to {target}")
            }
            "protection" => format!("Apply {target}'s protection: prevent the remaining damage"),
            "color" => format!("Apply color prevention from {name}: prevent the remaining damage"),
            "shield" => format!(
                "Apply {name}'s shield ({} remaining) to damage to {target}",
                option
                    .shield_remaining
                    .ok_or(Error::InvalidVisibleReference)?
            ),
            _ => return Err(Error::UnsupportedPrompt),
        });
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}
type ProjectedCrystalV22 = (
    HumanVisibleStateV1<HumanCrystalExtensionsV22>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &CrystalObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedCrystalV22, Error> {
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
    for row in &observation.static_sources {
        handles.insert(&row.source)?;
    }
    if let Some(row) = observation.prevention_menu.as_ref() {
        handles.insert(&row.source)?;
        handles.insert(&row.recipient)?;
        for option in &row.options {
            handles.insert(&option.source)?;
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
    for row in &initial.extensions.static_sources {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    if let Some(row) = initial.extensions.prevention_menu.as_ref() {
        unary
            .entry(row.recipient.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
        for option in &row.options {
            unary
                .entry(option.source.handle.clone())
                .or_default()
                .push(visible::without_handles(option)?);
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

struct BoundCrystalV22 {
    authority: AuthorityV22,
    expected: FastActorDecisionV1,
    binding: CrystalDecisionBindingV22,
    public: crate::crystal_public_v1::CrystalObservationV1,
    visible: HumanDecisionCrystalV22,
    engine_indexes: Vec<u32>,
}
pub struct HumanCrystalProjectorV22 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundCrystalV22>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV22)>,
}
impl HumanCrystalProjectorV22 {
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
        session: &HumanCrystalSessionV22,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionCrystalV22, Error> {
        if expected.acting_player != self.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .crystal_current_decision_input_v22(expected, self.human_seat)
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
        let visible = HumanDecisionCrystalV22 {
            schema: "mtg-kernel-human-crystal-decision/v22".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundCrystalV22 {
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
        session: &mut HumanCrystalSessionV22,
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
            .crystal_validate_bound_decision_v22(
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
#[path = "crystal_v22_tests.rs"]
mod tests;
