//! Sealed GameOutcome successor. Complete typed authority stays backend-only.
use super::visible::Handles;
use super::*;
use crate::game_outcome_observation_v41::GameOutcomeObservationV41;
use crate::rl::ActionSemanticV1;
use crate::rl_session::game_outcome_v41::GameOutcomeDecisionBindingV41;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};

type Error = HumanDecisionErrorV1;

// Backend-only authority survives owning restore. Snapshots never capture it.
#[derive(Clone)]
struct AuthorityV41 {
    owner: std::sync::Arc<()>,
    generation: u64,
}
impl PartialEq for AuthorityV41 {
    fn eq(&self, other: &Self) -> bool {
        self.generation == other.generation && std::sync::Arc::ptr_eq(&self.owner, &other.owner)
    }
}
impl Eq for AuthorityV41 {}
pub struct HumanGameOutcomeSessionV41 {
    inner: FastActorSessionV1,
    authority: AuthorityV41,
    human_seat: PlayerSeatV1,
}
#[derive(Clone)]
pub struct HumanGameOutcomeSnapshotV41 {
    snapshot: FastActorSessionSnapshotV1,
    owner: std::sync::Arc<()>,
}
impl HumanGameOutcomeSessionV41 {
    pub fn from_session(
        session: FastActorSessionV1,
        human_seat: PlayerSeatV1,
    ) -> Result<Self, crate::rl::RlContractError> {
        Ok(Self {
            inner: session.into_game_outcome_v41()?,
            human_seat,
            authority: AuthorityV41 {
                owner: std::sync::Arc::new(()),
                generation: 0,
            },
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v41(&self) -> HumanGameOutcomeSnapshotV41 {
        HumanGameOutcomeSnapshotV41 {
            snapshot: self.inner.snapshot_v1(),
            owner: self.authority.owner.clone(),
        }
    }
    pub fn restore_v41(&mut self, snapshot: &HumanGameOutcomeSnapshotV41) -> Result<(), Error> {
        if !std::sync::Arc::ptr_eq(&self.authority.owner, &snapshot.owner) {
            return Err(Error::StaleDecision);
        }
        let next = self
            .authority
            .generation
            .checked_add(1)
            .ok_or(Error::PromptSequenceExhausted)?;
        self.inner
            .game_outcome_restore_v41(&snapshot.snapshot)
            .map_err(|_| Error::StaleDecision)?;
        self.authority.generation = next;
        Ok(())
    }
    /// Public typed transport, not the safe Human DTO and not scorer admission.
    pub fn public_observation(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<
        crate::game_outcome_observation_v41::GameOutcomeObservationV41,
        crate::rl::RlContractError,
    > {
        if expected.acting_player != self.human_seat {
            return Err(crate::rl::RlContractError(
                "GameOutcome Human view is fixed to its selected seat".into(),
            ));
        }
        self.inner
            .game_outcome_current_decision_input_v41(expected, self.human_seat)
            .map(|(observation, _, _)| observation)
            .map_err(|_| crate::rl::RlContractError("stale GameOutcome public decision".into()))
    }
    /// Host-only policy callback. Opponent private rosters never enter a Human DTO.
    pub fn step_opponent_with<F>(
        &mut self,
        expected: FastActorDecisionV1,
        mut policy: F,
    ) -> Result<FastActorResponseV1, crate::rl::RlContractError>
    where
        F: FnMut(
            &GameOutcomeObservationV41,
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
            .game_outcome_current_decision_input_v41(expected, expected.acting_player)
            .map_err(|_| crate::rl::RlContractError("stale opponent input".into()))?;
        let index = policy(&observation, &actions)?;
        if index as usize >= actions.len() {
            return Err(crate::rl::RlContractError(
                "opponent action out of range".into(),
            ));
        }
        self.inner
            .game_outcome_validate_bound_decision_v41(expected, expected.acting_player, &binding)
            .map_err(|_| crate::rl::RlContractError("opponent input changed".into()))?;
        self.inner
            .step(expected.episode_id, expected.step, index)
            .map_err(|error| crate::rl::RlContractError(format!("opponent execution:{error:?}")))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionGameOutcomeV41 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanGameOutcomeExtensionsV41>,
    pub actions: Vec<HumanLegalActionV1>,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanGameOutcomeSourceV41 {
    pub source: HumanCardRefV1,
    pub printed_rules_active: bool,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanGameOutcomeExtensionsV41 {
    pub predecessor: super::akroma_v40::HumanAkromaExtensionsV40,
    pub sources: Vec<HumanGameOutcomeSourceV41>,
    pub cannot_lose: [bool; 2],
    pub cannot_win: [bool; 2],
}
type ProjectedGameOutcomeV41 = (
    HumanVisibleStateV1<HumanGameOutcomeExtensionsV41>,
    Vec<HumanLegalActionV1>,
    Vec<u32>,
);
fn project_base(
    observation: &GameOutcomeObservationV41,
    actions: &[ActionSemanticV1],
    human: PlayerSeatV1,
    families: &[crate::akroma_family_public_v40::FamilyProjectionV40],
) -> Result<ProjectedGameOutcomeV41, Error> {
    observation
        .validate_artifact()
        .map_err(|_| Error::InvalidVisibleReference)?;
    let base = &observation.predecessor;
    let (old, actions, indexes) = super::akroma_v40::project_base(base, actions, human, families)?;
    // Current battlefield sources already have handles before historical rows are inserted.
    let handles = Handles::from_parts(
        &base.projection,
        &base.own_hand,
        &base.known_hand_cards,
        &base.known_library_cards,
        base.extensions.decision_local_library.as_ref(),
    )?;
    let mut sources = observation
        .protection
        .sources
        .iter()
        .map(|row| {
            Ok(HumanGameOutcomeSourceV41 {
                source: handles.get(&row.source)?.clone(),
                printed_rules_active: row.printed_rules_active,
            })
        })
        .collect::<Result<Vec<_>, Error>>()?;
    sources.sort_by(|a, b| a.source.handle.cmp(&b.source.handle));
    Ok((
        HumanVisibleStateV1 {
            public: old.public,
            own_hand: old.own_hand,
            known_library_cards: old.known_library_cards,
            known_hand_cards: old.known_hand_cards,
            extensions: HumanGameOutcomeExtensionsV41 {
                predecessor: old.extensions,
                sources,
                cannot_lose: observation.protection.cannot_lose,
                cannot_win: observation.protection.cannot_win,
            },
            policy_context: old.policy_context,
            known_face_down_cards: old.known_face_down_cards,
            poison_counters: old.poison_counters,
            poison_prevention: old.poison_prevention,
            creatures_attacked_this_turn: old.creatures_attacked_this_turn,
            ninja_emblems: old.ninja_emblems,
            restricted_mana: old.restricted_mana,
        },
        actions,
        indexes,
    ))
}
struct BoundGameOutcomeV41 {
    authority: AuthorityV41,
    expected: FastActorDecisionV1,
    binding: GameOutcomeDecisionBindingV41,
    public: crate::game_outcome_observation_v41::GameOutcomeObservationV41,
    visible: HumanDecisionGameOutcomeV41,
    engine_indexes: Vec<u32>,
}
pub struct HumanGameOutcomeProjectorV41 {
    human_seat: PlayerSeatV1,
    next_prompt_seq: u64,
    pending: Option<BoundGameOutcomeV41>,
    accepted: Option<(HumanActionReceiptV1, AuthorityV41)>,
}
impl HumanGameOutcomeProjectorV41 {
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
        session: &HumanGameOutcomeSessionV41,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionGameOutcomeV41, Error> {
        if expected.acting_player != self.human_seat || self.human_seat != session.human_seat {
            return Err(Error::NotHumanTurn);
        }
        let (observation, actions, binding) = session
            .inner
            .game_outcome_current_decision_input_v41(expected, self.human_seat)
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
        let (state, actions, engine_indexes) = project_base(
            &observation,
            &actions,
            self.human_seat,
            &binding.predecessor.families,
        )?;
        let visible = HumanDecisionGameOutcomeV41 {
            schema: "mtg-kernel-human-game_outcome-decision/v41".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state,
            actions,
        };
        self.pending = Some(BoundGameOutcomeV41 {
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
        session: &mut HumanGameOutcomeSessionV41,
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
            .game_outcome_validate_bound_decision_v41(
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

#[cfg(all(
    test,
    feature = "limited-fdn-fixtures",
    not(feature = "standard-magezero-fixtures")
))]
mod tests {
    use super::*;
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state_for};
    use crate::state::Zone;

    fn ready(actor: PlayerId) -> FastActorSessionV1 {
        let mut state = ready_state_for(actor);
        put(&mut state, actor, "Forest", Zone::Hand);
        put(&mut state, actor, "Mountain", Zone::Hand);
        put(&mut state, actor.opponent(), "Lightning Bolt", Zone::Hand);
        FastActorSessionV1::from_akroma_v40_fixture_state(state)
    }

    #[test]
    fn held_prompt_and_receipt_are_stale_after_each_restore_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut session =
                HumanGameOutcomeSessionV41::from_session(ready(actor), actor.into()).unwrap();
            let snapshot = session.snapshot_v41();
            let mut projector = HumanGameOutcomeProjectorV41::new(actor.into());
            for _ in 0..2 {
                let FastActorResponseV1::Decision(expected) = session.current_response() else {
                    panic!("actual priority")
                };
                let menu = projector.project_current(&session, expected).unwrap();
                assert_eq!(menu.state.extensions.cannot_lose, [false; 2]);
                let serialized = serde_json::to_string(&menu).unwrap();
                assert!(!serialized.contains("Lightning Bolt"));
                for field in [
                    "arena_id",
                    "private_snapshot_json",
                    "event_history",
                    "printed_meaning",
                ] {
                    assert!(!serialized.contains(field));
                }
                let land = menu
                    .actions
                    .iter()
                    .find(|row| row.label.contains("Play") && row.label.contains("Forest"))
                    .unwrap();
                let request = HumanActionRequestV1 {
                    prompt_seq: menu.prompt_seq,
                    action_index: land.action_index,
                };
                let receipt = projector.submit(&mut session, request.clone()).unwrap();
                assert_eq!(
                    projector.submit(&mut session, request.clone()).unwrap(),
                    receipt
                );
                session.restore_v41(&snapshot).unwrap();
                let before = session.inner.privileged_core_environment_hash();
                assert_eq!(
                    projector.submit(&mut session, request),
                    Err(Error::StaleDecision)
                );
                assert_eq!(session.inner.privileged_core_environment_hash(), before);
            }
            let foreign = HumanGameOutcomeSessionV41::from_session(ready(actor), actor.into())
                .unwrap()
                .snapshot_v41();
            assert_eq!(session.restore_v41(&foreign), Err(Error::StaleDecision));
        }
    }
}
