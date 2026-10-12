//! Sealed ZulAshur successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::rl::ActionSemanticV1;
use crate::rl_session::zul_ashur_v35::ZulAshurDecisionBindingV35;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use crate::zul_ashur_observation_v1::ZulAshurObservationV1;
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV35 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV35 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV35 {}
pub struct HumanZulAshurSessionV35 {
    inner: FastActorSessionV1,
    authority: AuthorityV35,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanZulAshurSnapshotV35 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanZulAshurSessionV35 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_zul_ashur_v35()?,
            human_seat,
            authority: AuthorityV35 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v35(&self) -> HumanZulAshurSnapshotV35 {
        HumanZulAshurSnapshotV35 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v35(&mut self, snapshot: &HumanZulAshurSnapshotV35) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .zul_ashur_restore_v35(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::zul_ashur_observation_v1::ZulAshurObservationV1, crate::rl::RlContractError>
    {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "ZulAshur Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .zul_ashur_current_decision_input_v35(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale ZulAshur public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &ZulAshurObservationV1,
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
            .zul_ashur_current_decision_input_v35(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .zul_ashur_validate_bound_decision_v35(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionZulAshurV35 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanZulAshurExtensionsV35>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanZulAshurExtensionsV35 {
    pub predecessor: HumanExtensionsV9,
    pub permissions: Vec<HumanZulAshurPermissionV35>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanZulAshurPermissionV35 {
    pub source_at_activation: HumanCardRefV1,
    pub original_graveyard_card: HumanCardRefV1,
    pub holder: PlayerSeatV1,
    pub granted_this_turn: bool,
    pub original_card_in_graveyard: bool,
    pub expired: bool,
}
fn parts(observation: &ZulAshurObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &ZulAshurObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanZulAshurExtensionsV35>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let permissions = observation
        .facts
        .iter()
        .map(|row| -> Result<_, Error> {
            Ok(HumanZulAshurPermissionV35 {
                source_at_activation: handles.get(&row.source_at_activation)?.clone(),
                original_graveyard_card: handles.get(&row.original_target)?.clone(),
                holder: row.holder,
                granted_this_turn: row.granted_this_turn,
                original_card_in_graveyard: row.original_card_in_graveyard,
                expired: row.expired,
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanZulAshurExtensionsV35 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            permissions,
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
type ProjectedZulAshurV35 = (
    HumanVisibleStateV1<HumanZulAshurExtensionsV35>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &ZulAshurObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedZulAshurV35, Error> {
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
    for permission in &observation.facts {
        handles.insert(&permission.source_at_activation)?;
        handles.insert(&permission.original_target)?;
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
    for row in &initial.extensions.permissions {
        let trait_value = visible::without_handles(row)?;
        unary
            .entry(row.source_at_activation.handle.clone())
            .or_default()
            .push(format!("zul_permission_source:{trait_value}"));
        unary
            .entry(row.original_graveyard_card.handle.clone())
            .or_default()
            .push(format!("zul_permission_target:{trait_value}"));
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

struct BoundZulAshurV35 {
    authority: AuthorityV35,
    expected: FastActorDecisionV1,
    binding: ZulAshurDecisionBindingV35,
    public: crate::zul_ashur_observation_v1::ZulAshurObservationV1,
    visible: HumanDecisionZulAshurV35,
    engine_indexes: Vec<u32>,
}
pub struct HumanZulAshurProjectorV35 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundZulAshurV35>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV35)>,
}
impl HumanZulAshurProjectorV35 {
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
        session: &HumanZulAshurSessionV35,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionZulAshurV35, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .zul_ashur_current_decision_input_v35(expected, self.human_seat)
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
        let visible = HumanDecisionZulAshurV35 {
            schema: "mtg-kernel-human-zul_ashur-decision/v35".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundZulAshurV35 {
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
        session: &mut HumanZulAshurSessionV35,
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
            .zul_ashur_validate_bound_decision_v35(
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

fn label(
    action: &ActionSemanticV1,
    observation: &ZulAshurObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    match action {
        ActionSemanticV1::ActivateAbility {
            actor,
            source,
            ability_index: 0,
            granted_ability,
            ..
        } if granted_ability.is_none()
            && observation.roots.iter().any(|r| {
                r.card_id == source.card_db_id
                    && r.root == crate::rules_meaning_zul_ashur_v1::RootV1::PrintedActivation
            }) =>
        {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Activate {} (tap): you may cast one targeted Zombie creature card from your graveyard this turn, with ordinary timing and costs",handles.name(source)?));
        }
        ActionSemanticV1::CastSpell { actor, source }
            if observation.roots.iter().any(|r| {
                r.card_id == source.card_db_id
                    && r.root == crate::rules_meaning_zul_ashur_v1::RootV1::PrintedCreature
            }) =>
        {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Begin casting {} (printed cost {{1}}{{B}}): legendary 2/2 Zombie Warlock with Ward pay 2 life",handles.name(source)?));
        }
        ActionSemanticV1::CastSpell { actor, source }
            if source.zone == crate::state::Zone::Graveyard =>
        {
            if *actor != human
                || !observation.facts.iter().any(|p| {
                    p.original_target == *source
                        && p.holder == human
                        && p.granted_this_turn
                        && p.original_card_in_graveyard
                        && !p.expired
                })
            {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Cast {} from your graveyard with ordinary timing and costs; permission ends this turn",handles.name(source)?));
        }
        _ => {}
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}
#[cfg(test)]
#[path = "zul_ashur_v35_tests.rs"]
mod tests;
