//! Human successor over the owning variable-cast session. Browser DTOs carry
//! canonical visible handles and wide ordinals, never backend commitments.
use super::life_ward_v8::{HumanExtensionsV8, ProjectionPartsV8};
use super::visible::Handles;
use super::{visible, HumanCardRefV1, HumanDecisionErrorV1 as Error, HumanVisibleStateV1};
use crate::policy_observation_variable_cast_v1::{
    ObservationVariableCastV1, VariableCastContextV1,
};
use crate::rl::PlayerSeatV1;
use crate::rl_session::variable_cast_v1::{
    VariableCastBindingV1, VariableCastRequestV1, VariableCastResponseV1, VariableCastSessionV1,
};
use crate::rl_variable_cast_v1::ActionSemanticVariableCastV1;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const HUMAN_VARIABLE_CAST_SCHEMA_V1: &str = "mtg-kernel-human-variable-cast/v1";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum HumanVariableCastContextV1 {
    Kicker {
        source: HumanCardRefV1,
        controller: PlayerSeatV1,
        legal_pay_choices: Vec<bool>,
    },
    Targets {
        source: HumanCardRefV1,
        controller: PlayerSeatV1,
        kicked: bool,
        selected_targets: Vec<HumanCardRefV1>,
        ordered: bool,
        selected_count: u64,
        minimum: u64,
        maximum: Option<u64>,
        legal_targets: Vec<HumanCardRefV1>,
        can_finish: bool,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanVariableCastExtensionsV1 {
    #[serde(flatten)]
    pub predecessor: HumanExtensionsV8,
    pub attacked_this_turn: [bool; 2],
    pub variable_cast: Option<HumanVariableCastContextV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanVariableCastActionV1 {
    pub action_index: u64,
    pub label: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanVariableCastDecisionV1 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanVariableCastExtensionsV1>,
    pub actions: Vec<HumanVariableCastActionV1>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct HumanVariableCastRequestV1 {
    pub schema: String,
    pub prompt_seq: u64,
    pub action_index: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanVariableCastReceiptV1 {
    pub prompt_seq: u64,
    pub action_index: u64,
}

fn parts(observation: &ObservationVariableCastV1) -> ProjectionPartsV8<'_> {
    ProjectionPartsV8 {
        projection: &observation.projection,
        own_hand: &observation.own_hand,
        known_library_cards: &observation.known_library_cards,
        known_hand_cards: &observation.known_hand_cards,
        known_face_down_cards: &observation.known_face_down_cards,
        extensions: &observation.extensions.predecessor,
    }
}

fn context(
    observation: &ObservationVariableCastV1,
    handles: &Handles,
) -> Result<Option<HumanVariableCastContextV1>, Error> {
    observation
        .variable_cast
        .as_ref()
        .map(|context| match context {
            VariableCastContextV1::Kicker {
                source,
                controller,
                legal_pay_choices,
            } => Ok(HumanVariableCastContextV1::Kicker {
                source: handles.get(source)?.clone(),
                controller: *controller,
                legal_pay_choices: legal_pay_choices.clone(),
            }),
            VariableCastContextV1::Targets {
                source,
                controller,
                kicked,
                selected,
                selected_count,
                minimum,
                maximum,
                legal_choices,
                can_finish,
            } => {
                let selected_targets = selected
                    .iter()
                    .map(|item| handles.get(item).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                let mut legal_targets = legal_choices
                    .iter()
                    .map(|item| handles.get(item).cloned())
                    .collect::<Result<Vec<_>, _>>()?;
                visible::sort_visible(&mut legal_targets);
                Ok(HumanVariableCastContextV1::Targets {
                    source: handles.get(source)?.clone(),
                    controller: *controller,
                    kicked: *kicked,
                    selected_targets,
                    ordered: true,
                    selected_count: *selected_count,
                    minimum: *minimum,
                    maximum: *maximum,
                    legal_targets,
                    can_finish: *can_finish,
                })
            }
        })
        .transpose()
}

fn state(
    observation: &ObservationVariableCastV1,
    handles: &mut Handles,
) -> Result<HumanVisibleStateV1<HumanVariableCastExtensionsV1>, Error> {
    let common = super::life_ward_v8::project_state_parts(&parts(observation), handles)?;
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
        extensions: HumanVariableCastExtensionsV1 {
            predecessor: common.extensions,
            attacked_this_turn: observation.extensions.attacked_this_turn,
            variable_cast: context(observation, handles)?,
        },
    })
}

fn project(
    response: &VariableCastResponseV1,
    human: PlayerSeatV1,
    prompt_seq: u64,
) -> Result<(HumanVariableCastDecisionV1, Vec<u64>), Error> {
    let observation = &response.observation;
    if observation.acting_player != human {
        return Err(Error::NotHumanTurn);
    }
    if response.terminal || response.actions.is_empty() {
        return Err(Error::UnsupportedPrompt);
    }
    let extensions = &observation.extensions.predecessor;
    let mut handles = Handles::from_parts(
        &observation.projection,
        &observation.own_hand,
        &observation.known_hand_cards,
        &observation.known_library_cards,
        extensions.decision_local_library.as_ref(),
    )?;
    for row in &observation.projection.surface.stack {
        handles.insert(&row.source)?;
    }
    for row in &extensions.historical_public_sources {
        handles.insert(&row.source)?;
    }
    if let Some(context) = &observation.variable_cast {
        match context {
            VariableCastContextV1::Kicker { source, .. } => {
                handles.insert(source)?;
            }
            VariableCastContextV1::Targets {
                source,
                selected,
                legal_choices,
                ..
            } => {
                handles.insert(source)?;
                for item in selected.iter().chain(legal_choices) {
                    handles.insert(item)?;
                }
            }
        }
    }
    let initial = state(observation, &mut handles)?;
    let mut unary = BTreeMap::<String, Vec<String>>::new();
    for row in &initial.extensions.predecessor.ward_life_card_traits {
        unary
            .entry(row.source.handle.clone())
            .or_default()
            .push(visible::without_handles(row)?);
    }
    for rows in unary.values_mut() {
        rows.sort();
    }
    handles.canonicalize_parts(&initial, unary)?;
    let state = state(observation, &mut handles)?;
    let mut choices = Vec::new();
    for action in &response.actions {
        let label = match &action.semantic {
            ActionSemanticVariableCastV1::Predecessor { action } => {
                super::labels::label_parts_v8(action, &parts(observation), &handles, human)?
            }
            ActionSemanticVariableCastV1::ChooseVariableCastKicker { actor, source, pay } => {
                if *actor != human {
                    return Err(Error::UnsupportedPrompt);
                }
                format!(
                    "{} kicker for {}",
                    if *pay { "Pay" } else { "Decline" },
                    handles.name(source)?
                )
            }
            ActionSemanticVariableCastV1::ChooseVariableCastTarget {
                actor,
                source,
                selected_count,
                target,
                ..
            } => {
                if *actor != human {
                    return Err(Error::UnsupportedPrompt);
                }
                format!(
                    "Choose {} for {} after {} selected targets",
                    handles.name(target)?,
                    handles.name(source)?,
                    selected_count
                )
            }
            ActionSemanticVariableCastV1::FinishVariableCastTargets {
                actor,
                source,
                selected_count,
            } => {
                if *actor != human {
                    return Err(Error::UnsupportedPrompt);
                }
                format!(
                    "Finish {} with {} selected targets",
                    handles.name(source)?,
                    selected_count
                )
            }
        };
        choices.push((label, action.ordinal));
    }
    choices.sort_by(|a, b| a.0.cmp(&b.0));
    if choices.windows(2).any(|pair| pair[0].0 == pair[1].0) {
        return Err(Error::UnsupportedPrompt);
    }
    let mapping = choices.iter().map(|(_, ordinal)| *ordinal).collect();
    let actions = choices
        .into_iter()
        .enumerate()
        .map(|(index, (label, _))| {
            Ok(HumanVariableCastActionV1 {
                action_index: u64::try_from(index).map_err(|_| Error::InvalidAction)?,
                label,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    Ok((
        HumanVariableCastDecisionV1 {
            schema: HUMAN_VARIABLE_CAST_SCHEMA_V1.into(),
            human_seat: human,
            prompt_seq,
            state,
            actions,
        },
        mapping,
    ))
}

#[derive(Clone)]
struct Bound {
    binding: VariableCastBindingV1,
    visible: HumanVariableCastDecisionV1,
    mapping: Vec<u64>,
}

/// Private backend ownership prevents callers from changing state underneath
/// a Human prompt. Only the canonical visible DTO crosses this boundary.
#[derive(Clone)]
pub struct HumanVariableCastSessionV1 {
    session: VariableCastSessionV1,
    human: PlayerSeatV1,
    prompt_seq: u64,
    pending: Option<Bound>,
    accepted: Option<(HumanVariableCastRequestV1, HumanVariableCastReceiptV1)>,
}

#[derive(Clone)]
pub struct HumanVariableCastSnapshotV1 {
    backend: crate::rl_session::variable_cast_v1::VariableCastSnapshotV1,
    human: PlayerSeatV1,
    prompt_seq: u64,
}

impl HumanVariableCastSessionV1 {
    /// Trusted host policy receives the acting opponent's backend projection.
    /// No opponent roster, request binding or action enters the Human DTO.
    pub fn apply_opponent_policy(
        &mut self,
        choose: impl FnOnce(&VariableCastResponseV1) -> Result<u64, Error>,
    ) -> Result<(), Error> {
        let response = self.session.current().map_err(|_| Error::ExecutionFailed)?;
        if response.terminal || response.observation.acting_player == self.human {
            return Err(Error::UnsupportedPrompt);
        }
        let ordinal = choose(&response)?;
        let mut next = self.clone();
        next.session
            .submit(VariableCastRequestV1 {
                binding: response.binding,
                ordinal,
            })
            .map_err(|_| Error::ExecutionFailed)?;
        next.pending = None;
        next.accepted = None;
        *self = next;
        Ok(())
    }

    pub fn save(&self) -> Result<HumanVariableCastSnapshotV1, Error> {
        Ok(HumanVariableCastSnapshotV1 {
            backend: self.session.save().map_err(|_| Error::ExecutionFailed)?,
            human: self.human,
            prompt_seq: self.prompt_seq,
        })
    }

    /// Preserve this owner's prompt high-water when restoring older state.
    /// The next prompt therefore cannot collide with a future prompt already
    /// issued before rollback, even when its action index is still legal.
    pub fn restore(&mut self, snapshot: HumanVariableCastSnapshotV1) -> Result<(), Error> {
        if snapshot.human != self.human {
            return Err(Error::InvalidVisibleReference);
        }
        let mut next = self.clone();
        next.prompt_seq = next.prompt_seq.max(snapshot.prompt_seq);
        next.prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        next.session
            .restore(snapshot.backend)
            .map_err(|_| Error::ExecutionFailed)?;
        next.pending = None;
        next.accepted = None;
        *self = next;
        Ok(())
    }
    pub fn new(session: VariableCastSessionV1, human: PlayerSeatV1) -> Self {
        Self {
            session,
            human,
            prompt_seq: 0,
            pending: None,
            accepted: None,
        }
    }

    pub fn current(&mut self) -> Result<HumanVariableCastDecisionV1, Error> {
        let response = self.session.current().map_err(|_| Error::ExecutionFailed)?;
        if let Some(pending) = &self.pending {
            if pending.binding == response.binding {
                let (fresh, mapping) = project(&response, self.human, pending.visible.prompt_seq)?;
                if fresh != pending.visible || mapping != pending.mapping {
                    return Err(Error::StaleDecision);
                }
                return Ok(fresh);
            }
        }
        let prompt_seq = self
            .prompt_seq
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        let (visible, mapping) = project(&response, self.human, prompt_seq)?;
        self.prompt_seq = prompt_seq;
        self.pending = Some(Bound {
            binding: response.binding,
            visible: visible.clone(),
            mapping,
        });
        Ok(visible)
    }

    pub fn submit(
        &mut self,
        request: HumanVariableCastRequestV1,
    ) -> Result<HumanVariableCastReceiptV1, Error> {
        self.session.current().map_err(|_| Error::ExecutionFailed)?;
        if let Some((accepted, receipt)) = &self.accepted {
            if accepted == &request {
                return Ok(receipt.clone());
            }
            if accepted.prompt_seq == request.prompt_seq {
                return Err(Error::ConflictingRetry);
            }
        }
        let mut next = self.clone();
        let current = next.current()?;
        if request.schema != HUMAN_VARIABLE_CAST_SCHEMA_V1
            || request.prompt_seq != current.prompt_seq
        {
            return Err(Error::StaleDecision);
        }
        let index = usize::try_from(request.action_index).map_err(|_| Error::InvalidAction)?;
        let bound = next.pending.as_ref().ok_or(Error::StaleDecision)?;
        let ordinal = *bound.mapping.get(index).ok_or(Error::InvalidAction)?;
        next.session
            .submit(VariableCastRequestV1 {
                binding: bound.binding.clone(),
                ordinal,
            })
            .map_err(|_| Error::ExecutionFailed)?;
        let receipt = HumanVariableCastReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        next.pending = None;
        next.accepted = Some((request, receipt.clone()));
        *self = next;
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::Zone;

    fn ordinary(actor: PlayerId, reversed: bool, hidden: &str) -> HumanVariableCastSessionV1 {
        let mut state = ready_state();
        state.active_player = actor;
        state.priority_player = actor;
        state.players[actor.index()].mana_pool[crate::mana::ManaColor::U.pool_index()] = 2;
        put(&mut state, actor.opponent(), hidden, Zone::Hand);
        put(&mut state, actor.opponent(), hidden, Zone::Library);
        for name in if reversed {
            ["Snap", "Forest"]
        } else {
            ["Forest", "Snap"]
        } {
            put(&mut state, actor, name, Zone::Hand);
        }
        put(
            &mut state,
            actor.opponent(),
            "Llanowar Elves",
            Zone::Battlefield,
        );
        HumanVariableCastSessionV1::new(
            VariableCastSessionV1::from_state(11062, state).unwrap(),
            actor.into(),
        )
    }

    fn request(decision: &HumanVariableCastDecisionV1, needle: &str) -> HumanVariableCastRequestV1 {
        HumanVariableCastRequestV1 {
            schema: HUMAN_VARIABLE_CAST_SCHEMA_V1.into(),
            prompt_seq: decision.prompt_seq,
            action_index: decision
                .actions
                .iter()
                .find(|action| action.label.contains(needle))
                .unwrap()
                .action_index,
        }
    }

    #[test]
    fn registered_hidden_substitution_and_arena_renumbering_keep_human_dto_equal() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut first = ordinary(actor, false, "Snap");
            let mut second = ordinary(actor, true, "Lightning Bolt");
            let visible = first.current().unwrap();
            assert_eq!(visible, second.current().unwrap());
            let encoded = serde_json::to_string(&visible).unwrap();
            for forbidden in [
                "arena_id",
                "zone_change_count",
                "card_db_hash",
                "commitment",
                "visible_projection_hash",
                "policy_action",
            ] {
                assert!(!encoded.contains(forbidden), "{forbidden}");
            }
            assert!(encoded.contains("attacked_this_turn"));
            assert!(encoded.contains("ward_life_card_traits"));
        }
    }

    #[test]
    fn human_wide_request_readonly_refusal_and_exact_once_retry() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session = ordinary(actor, false, "Snap");
            let before = session.current().unwrap();
            let bad = HumanVariableCastRequestV1 {
                schema: HUMAN_VARIABLE_CAST_SCHEMA_V1.into(),
                prompt_seq: before.prompt_seq,
                action_index: u64::MAX,
            };
            assert_eq!(session.submit(bad), Err(Error::InvalidAction));
            assert_eq!(session.current().unwrap(), before);
            let play = request(&before, "Play Forest");
            let receipt = session.submit(play.clone()).unwrap();
            let after = session.current().unwrap();
            assert_eq!(session.submit(play.clone()).unwrap(), receipt);
            assert_eq!(session.current().unwrap(), after);
            let mut conflict = play;
            conflict.action_index += 1;
            assert_eq!(session.submit(conflict), Err(Error::ConflictingRetry));
            assert_eq!(session.current().unwrap(), after);
            session.submit(request(&after, "Pass priority")).unwrap();
            assert_eq!(session.current(), Err(Error::NotHumanTurn));
            let opponent = session.session.current().unwrap();
            assert_eq!(opponent.observation.acting_player, actor.opponent().into());
            assert_eq!(
                session.apply_opponent_policy(|_| Ok(u64::MAX)),
                Err(Error::ExecutionFailed)
            );
            assert_eq!(session.session.current().unwrap(), opponent);
            session
                .apply_opponent_policy(|response| {
                    Ok(response
                        .actions
                        .iter()
                        .find(|action| {
                            matches!(
                                action.semantic,
                                ActionSemanticVariableCastV1::Predecessor {
                                    action: crate::rl::ActionSemanticV1::Pass { .. }
                                }
                            )
                        })
                        .unwrap()
                        .ordinal)
                })
                .unwrap();
            assert_eq!(session.current().unwrap().human_seat, actor.into());
        }
    }

    #[test]
    fn human_snapshot_rebuilds_handles_and_invalidates_old_prompt_without_mutation() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session = ordinary(actor, false, "Snap");
            let before = session.current().unwrap();
            let old = request(&before, "Play Forest");
            let snapshot = session.save().unwrap();
            session.submit(old.clone()).unwrap();
            let future = session.current().unwrap();
            let future_request = request(&future, "Pass priority");
            assert!(future.prompt_seq > before.prompt_seq);
            session.restore(snapshot).unwrap();
            let fresh = session.current().unwrap();
            assert_eq!(fresh.state, before.state);
            assert_eq!(fresh.actions, before.actions);
            assert!(fresh.prompt_seq > future.prompt_seq);
            // The old future request still names a legal ordinal. Its prompt
            // must refuse because rollback restored a different decision.
            assert_eq!(
                future_request.action_index,
                request(&fresh, "Pass priority").action_index
            );
            assert_eq!(session.submit(future_request), Err(Error::StaleDecision));
            assert_eq!(session.current().unwrap(), fresh);
            assert_eq!(session.submit(old), Err(Error::StaleDecision));
            assert_eq!(session.current().unwrap(), fresh);
        }
    }

    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    #[test]
    fn actual_registered_divine_human_true_only_zero_and_three_hundred_restore_finish() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            for total in [0usize, 300] {
                let mut state = ready_state();
                state.active_player = actor;
                state.priority_player = actor;
                state.players[actor.index()].mana_pool[crate::mana::ManaColor::W.pool_index()] = 4;
                put(&mut state, actor, "Divine Resilience", Zone::Hand);
                for _ in 0..total {
                    put(&mut state, actor, "Llanowar Elves", Zone::Battlefield);
                }
                let mut session = HumanVariableCastSessionV1::new(
                    VariableCastSessionV1::from_state(11062, state).unwrap(),
                    actor.into(),
                );
                let cast = request(
                    &session.current().unwrap(),
                    "Begin casting Divine Resilience",
                );
                session.submit(cast).unwrap();
                let kicker = session.current().unwrap();
                let Some(HumanVariableCastContextV1::Kicker {
                    legal_pay_choices, ..
                }) = &kicker.state.extensions.variable_cast
                else {
                    panic!("human kicker");
                };
                assert_eq!(
                    *legal_pay_choices,
                    if total == 0 {
                        vec![true]
                    } else {
                        vec![false, true]
                    }
                );
                assert_eq!(kicker.actions.len(), legal_pay_choices.len());
                session.submit(request(&kicker, "Pay kicker")).unwrap();
                for count in 0..total {
                    let targets = session.current().unwrap();
                    let Some(HumanVariableCastContextV1::Targets {
                        selected_count,
                        maximum,
                        ..
                    }) = targets.state.extensions.variable_cast.as_ref()
                    else {
                        panic!("human targets");
                    };
                    assert_eq!(*selected_count, u64::try_from(count).unwrap());
                    assert_eq!(*maximum, None);
                    session
                        .submit(request(&targets, "Choose Llanowar Elves"))
                        .unwrap();
                }
                let finished = session.current().unwrap();
                let Some(HumanVariableCastContextV1::Targets {
                    selected_count,
                    selected_targets,
                    minimum,
                    maximum,
                    can_finish,
                    ..
                }) = &finished.state.extensions.variable_cast
                else {
                    panic!("human completed prefix");
                };
                assert_eq!(*selected_count, u64::try_from(total).unwrap());
                assert_eq!(selected_targets.len(), total);
                assert_eq!((*minimum, *maximum, *can_finish), (0, None, true));
                let snapshot = session.save().unwrap();
                session.restore(snapshot).unwrap();
                let restored = session.current().unwrap();
                assert_eq!(restored.state, finished.state);
                assert_eq!(restored.actions, finished.actions);
                session
                    .submit(request(&restored, "Finish Divine Resilience"))
                    .unwrap();
                assert!(session
                    .session
                    .current()
                    .unwrap()
                    .observation
                    .variable_cast
                    .is_none());
            }
        }
    }
}
