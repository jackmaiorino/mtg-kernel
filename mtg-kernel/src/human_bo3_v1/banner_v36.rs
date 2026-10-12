//! Sealed Banner successor. Complete typed authority stays backend-only.
use super::visible::{self, Handles};
use super::*;
use crate::banner_observation_v1::BannerObservationV1;
use crate::rl::ActionSemanticV1;
use crate::rl_session::banner_v36::BannerDecisionBindingV36;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use std::collections::BTreeMap;
type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV36 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV36 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV36 {}
pub struct HumanBannerSessionV36 {
    inner: FastActorSessionV1,
    authority: AuthorityV36,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanBannerSnapshotV36 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanBannerSessionV36 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_banner_v36()?,
            human_seat,
            authority: AuthorityV36 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v36(&self) -> HumanBannerSnapshotV36 {
        HumanBannerSnapshotV36 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v36(&mut self, snapshot: &HumanBannerSnapshotV36) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .banner_restore_v36(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::banner_observation_v1::BannerObservationV1, crate::rl::RlContractError> {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "Banner Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .banner_current_decision_input_v36(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale Banner public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &BannerObservationV1,
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
            .banner_current_decision_input_v36(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .banner_validate_bound_decision_v36(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionBannerV36 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanBannerExtensionsV36>,
    pub actions: Vec<HumanLegalActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBannerExtensionsV36 {
    pub predecessor: HumanExtensionsV9,
    pub entry_choice: Option<HumanBannerEntryV36>,
    pub chosen_colors: Vec<HumanBannerColorV36>,
    pub prior_entry_colors: Vec<HumanBannerPriorColorV36>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBannerEntryV36 {
    pub handle: String,
    pub chooser: PlayerSeatV1,
    pub printed_name: String,
    pub colors: [crate::mana::ManaColor; 5],
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBannerPriorColorV36 {
    pub handle: String,
    pub chooser: PlayerSeatV1,
    pub printed_name: String,
    pub chosen_color: crate::mana::ManaColor,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanBannerColorV36 {
    pub source: HumanCardRefV1,
    pub chosen_color: crate::mana::ManaColor,
    pub active_printed_abilities: bool,
}
fn parts(observation: &BannerObservationV1) -> super::life_ward_v8::ProjectionPartsV8<'_> {
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
    observation: &BannerObservationV1,
    handles: &mut Handles,
    human: PlayerSeatV1,
) -> Result<HumanVisibleStateV1<HumanBannerExtensionsV36>, Error> {
    let old = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
    let seat = if human == PlayerSeatV1::P0 { 0 } else { 1 };
    let entry_choice = observation
        .entry_choice
        .as_ref()
        .map(|row| {
            let def = crate::card_def::CARD_DEFS
                .get(row.printed_card_db_id as usize)
                .ok_or(Error::InvalidVisibleReference)?;
            Ok(HumanBannerEntryV36 {
                handle: format!(
                    "entry{}",
                    row.entry_ordinal
                        .checked_add(1)
                        .ok_or(Error::InvalidVisibleReference)?
                ),
                chooser: row.chooser,
                printed_name: def.object_name.into(),
                colors: row.colors,
            })
        })
        .transpose()?;
    let prior_entry_colors = observation
        .prior_entry_colors
        .iter()
        .map(|row| {
            let def = crate::card_def::CARD_DEFS
                .get(row.printed_card_db_id as usize)
                .ok_or(Error::InvalidVisibleReference)?;
            Ok(HumanBannerPriorColorV36 {
                handle: format!(
                    "entry{}",
                    row.entry_ordinal
                        .checked_add(1)
                        .ok_or(Error::InvalidVisibleReference)?
                ),
                chooser: row.chooser,
                printed_name: def.object_name.into(),
                chosen_color: row.chosen_color,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    let chosen_colors = observation
        .chosen_colors
        .iter()
        .map(|row| {
            Ok(HumanBannerColorV36 {
                source: handles.get(&row.source)?.clone(),
                chosen_color: row.chosen_color,
                active_printed_abilities: row.active_printed_abilities,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok(HumanVisibleStateV1 {
        public: old.public,
        own_hand: old.own_hand,
        known_library_cards: old.known_library_cards,
        known_hand_cards: old.known_hand_cards,
        extensions: HumanBannerExtensionsV36 {
            predecessor: HumanExtensionsV9 {
                predecessor: old.extensions,
                own_attacked_this_turn: observation.attacked_this_turn[seat],
                opponent_attacked_this_turn: observation.attacked_this_turn[1 - seat],
            },
            entry_choice,
            chosen_colors,
            prior_entry_colors,
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
    observation: &BannerObservationV1,
    handles: &Handles,
    human: PlayerSeatV1,
) -> Result<String, Error> {
    if let ActionSemanticV1::ChooseEntryColorV36 {
        actor,
        entry_ordinal,
        color,
    } = action
    {
        let menu = observation
            .entry_choice
            .as_ref()
            .ok_or(Error::UnsupportedPrompt)?;
        if *actor != human
            || menu.chooser != human
            || *entry_ordinal != menu.entry_ordinal
            || !menu.colors.contains(color)
        {
            return Err(Error::UnsupportedPrompt);
        }
        let color = match color {
            crate::mana::ManaColor::W => "white",
            crate::mana::ManaColor::U => "blue",
            crate::mana::ManaColor::B => "black",
            crate::mana::ManaColor::R => "red",
            crate::mana::ManaColor::G => "green",
            crate::mana::ManaColor::C => return Err(Error::UnsupportedPrompt),
        };
        return Ok(format!(
            "Choose {color} for Heraldic Banner [entry{}] before it enters",
            entry_ordinal
                .checked_add(1)
                .ok_or(Error::InvalidVisibleReference)?
        ));
    }
    if let ActionSemanticV1::CastSpell { actor, source } = action {
        if observation
            .roots
            .iter()
            .any(|r| r.card_id == source.card_db_id)
        {
            if *actor != human {
                return Err(Error::UnsupportedPrompt);
            }
            return Ok(format!("Begin casting {} (printed cost {{3}}): choose a color as it enters; your creatures of that color get +1/+0; tap for one mana of that color",handles.name(source)?));
        }
    }
    super::labels::label_parts_v8(action, &parts(observation), handles, human)
}

type ProjectedBannerV36 = (
    HumanVisibleStateV1<HumanBannerExtensionsV36>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);

fn project_base(
    observation: &BannerObservationV1,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
) -> Result<ProjectedBannerV36, Error> {
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
    for row in &observation.chosen_colors {
        handles.insert(&row.source)?;
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
    for row in &initial.extensions.chosen_colors {
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

struct BoundBannerV36 {
    authority: AuthorityV36,
    expected: FastActorDecisionV1,
    binding: BannerDecisionBindingV36,
    public: crate::banner_observation_v1::BannerObservationV1,
    visible: HumanDecisionBannerV36,
    engine_indexes: Vec<u32>,
}
pub struct HumanBannerProjectorV36 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundBannerV36>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV36)>,
}
impl HumanBannerProjectorV36 {
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
        session: &HumanBannerSessionV36,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionBannerV36, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .banner_current_decision_input_v36(expected, self.human_seat)
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
        let visible = HumanDecisionBannerV36 {
            schema: "mtg-kernel-human-banner-decision/v36".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundBannerV36 {
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
        session: &mut HumanBannerSessionV36,
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
            .banner_validate_bound_decision_v36(pending.expected, self.human_seat, &pending.binding)
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
