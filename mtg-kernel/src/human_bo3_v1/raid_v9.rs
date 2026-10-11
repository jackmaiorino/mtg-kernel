//! Safe fixed-seat Raid successor. Rich observations, executable candidates
//! and predecessor bindings remain backend-only behind an owned session.
use super::*;
use crate::rl::RlContractError;
use crate::rl_session::{FastActorResponseV1, FastActorSessionSnapshotV1};
use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanExtensionsV9 {
    pub predecessor: HumanExtensionsV8,
    pub own_attacked_this_turn: bool,
    pub opponent_attacked_this_turn: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HumanDecisionV9 {
    pub schema: String,
    pub human_seat: PlayerSeatV1,
    pub prompt_seq: u64,
    pub state: HumanVisibleStateV1<HumanExtensionsV9>,
    pub actions: Vec<HumanLegalActionV1>,
}

pub struct HumanRaidSessionV9 {
    inner: FastActorSessionV1,
}

#[derive(Clone)]
pub struct HumanRaidSnapshotV9(FastActorSessionSnapshotV1);

impl HumanRaidSessionV9 {
    pub fn from_session(session: FastActorSessionV1) -> Result<Self, RlContractError> {
        Ok(Self {
            inner: session.into_human_v8()?,
        })
    }
    pub fn current_response(&self) -> FastActorResponseV1 {
        self.inner.current_response()
    }
    pub fn snapshot_v9(&self) -> HumanRaidSnapshotV9 {
        HumanRaidSnapshotV9(self.inner.snapshot_v1())
    }
    pub fn restore_v9(&mut self, snapshot: &HumanRaidSnapshotV9) {
        self.inner.restore_v1(&snapshot.0);
    }
}

struct BoundHumanV9 {
    expected: FastActorDecisionV1,
    public_projection: [u8; 32],
    predecessor_prompt_seq: u64,
    visible: HumanDecisionV9,
}

pub struct HumanDecisionProjectorV9 {
    human_seat: PlayerSeatV1,
    predecessor: HumanDecisionProjectorV8,
    next_prompt_seq: u64,
    pending: Option<BoundHumanV9>,
    accepted: Option<HumanActionReceiptV1>,
}

fn commitment(
    session: &HumanRaidSessionV9,
    expected: FastActorDecisionV1,
    human: PlayerSeatV1,
) -> Result<(crate::policy_observation_v9::ObservationV9, [u8; 32]), HumanDecisionErrorV1> {
    let observation = session
        .inner
        .human_raid_observation_v9(expected, human)
        .map_err(|_| HumanDecisionErrorV1::StaleDecision)?;
    let bytes = serde_json::to_vec(&observation)
        .map_err(|_| HumanDecisionErrorV1::InvalidVisibleReference)?;
    let digest = Sha256::digest(bytes).into();
    Ok((observation, digest))
}

impl HumanDecisionProjectorV9 {
    pub fn new(human_seat: PlayerSeatV1) -> Self {
        Self {
            human_seat,
            predecessor: HumanDecisionProjectorV8::new(human_seat),
            next_prompt_seq: 1,
            pending: None,
            accepted: None,
        }
    }

    pub fn project_current(
        &mut self,
        session: &HumanRaidSessionV9,
        expected: FastActorDecisionV1,
    ) -> Result<HumanDecisionV9, HumanDecisionErrorV1> {
        if expected.acting_player != self.human_seat {
            return Err(HumanDecisionErrorV1::NotHumanTurn);
        }
        let (observation, public_projection) = commitment(session, expected, self.human_seat)?;
        if let Some(pending) = &self.pending {
            if pending.expected == expected && pending.public_projection == public_projection {
                return Ok(pending.visible.clone());
            }
        }
        let next = self
            .next_prompt_seq
            .checked_add(1)
            .ok_or(HumanDecisionErrorV1::PromptSequenceExhausted)?;
        let previous = self.predecessor.project_current(&session.inner, expected)?;
        let predecessor_prompt_seq = previous.prompt_seq;
        let flags = observation.extensions.attacked_this_turn;
        let actor = usize::from(self.human_seat == PlayerSeatV1::P1);
        let state = previous.state;
        let visible = HumanDecisionV9 {
            schema: "mtg-kernel-human-decision/v9".into(),
            human_seat: self.human_seat,
            prompt_seq: self.next_prompt_seq,
            state: HumanVisibleStateV1 {
                public: state.public,
                own_hand: state.own_hand,
                known_library_cards: state.known_library_cards,
                known_hand_cards: state.known_hand_cards,
                policy_context: state.policy_context,
                extensions: HumanExtensionsV9 {
                    predecessor: state.extensions,
                    own_attacked_this_turn: flags[actor],
                    opponent_attacked_this_turn: flags[1 - actor],
                },
            },
            actions: previous.actions,
        };
        self.pending = Some(BoundHumanV9 {
            expected,
            public_projection,
            predecessor_prompt_seq,
            visible: visible.clone(),
        });
        self.next_prompt_seq = next;
        Ok(visible)
    }

    pub fn submit(
        &mut self,
        session: &mut HumanRaidSessionV9,
        request: HumanActionRequestV1,
    ) -> Result<HumanActionReceiptV1, HumanDecisionErrorV1> {
        if let Some(receipt) = &self.accepted {
            if receipt.prompt_seq == request.prompt_seq {
                return if receipt.action_index == request.action_index {
                    Ok(receipt.clone())
                } else {
                    Err(HumanDecisionErrorV1::ConflictingRetry)
                };
            }
        }
        let pending = self
            .pending
            .as_ref()
            .ok_or(HumanDecisionErrorV1::StaleDecision)?;
        if pending.visible.prompt_seq != request.prompt_seq {
            return Err(HumanDecisionErrorV1::StaleDecision);
        }
        let (_, current) = commitment(session, pending.expected, self.human_seat)?;
        if current != pending.public_projection {
            return Err(HumanDecisionErrorV1::StaleDecision);
        }
        self.predecessor.submit(
            &mut session.inner,
            HumanActionRequestV1 {
                prompt_seq: pending.predecessor_prompt_seq,
                action_index: request.action_index,
            },
        )?;
        let receipt = HumanActionReceiptV1 {
            prompt_seq: request.prompt_seq,
            action_index: request.action_index,
        };
        self.pending = None;
        self.accepted = Some(receipt.clone());
        Ok(receipt)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::{self, Action};
    use crate::ids::PlayerId;
    use crate::policy_observation_v6::tests::{put, ready_state};
    use crate::state::{Step, Zone};

    #[test]
    fn actual_history_projects_safely_refuses_stale_history_and_retries_once() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let mut state = ready_state();
            state.turn = 3;
            state.active_player = actor;
            state.priority_player = actor;
            state.step = Step::DeclareAttackers;
            let attacker = put(&mut state, actor, "Guttersnipe", Zone::Battlefield);
            engine::step(&mut state, Action::DeclareAttackers(vec![attacker])).unwrap();
            state.step = Step::Main2;
            state.engine.combat = Default::default();
            put(&mut state, actor, "Mountain", Zone::Hand);
            let mut session = HumanRaidSessionV9::from_session(
                FastActorSessionV1::from_human_v8_fixture_state(state),
            )
            .unwrap();
            let FastActorResponseV1::Decision(e) = session.current_response() else {
                panic!("live decision");
            };
            let mut projector = HumanDecisionProjectorV9::new(PlayerSeatV1::from(actor));
            let visible = projector.project_current(&session, e).unwrap();
            assert!(visible.state.extensions.own_attacked_this_turn);
            assert!(!visible.state.extensions.opponent_attacked_this_turn);
            let raw = serde_json::to_string(&visible).unwrap();
            for forbidden in [
                "arena_id",
                "visible_projection_hash",
                "card_db_hash",
                "public_projection",
                "source_contract",
            ] {
                assert!(!raw.contains(forbidden), "{forbidden}");
            }
            let snapshot = session.snapshot_v9();
            session.inner = session.inner.census_edited_clone_v1(|state| {
                state.attack_declaration_turn_v1 = None;
            });
            let request = HumanActionRequestV1 {
                prompt_seq: visible.prompt_seq,
                action_index: 0,
            };
            let before = session.inner.privileged_core_environment_hash();
            assert_eq!(
                projector.submit(&mut session, request.clone()),
                Err(HumanDecisionErrorV1::StaleDecision)
            );
            assert_eq!(session.inner.privileged_core_environment_hash(), before);
            session.restore_v9(&snapshot);
            let receipt = projector.submit(&mut session, request.clone()).unwrap();
            let after = session.inner.privileged_core_environment_hash();
            assert_eq!(projector.submit(&mut session, request).unwrap(), receipt);
            assert_eq!(session.inner.privileged_core_environment_hash(), after);
        }
    }
}
