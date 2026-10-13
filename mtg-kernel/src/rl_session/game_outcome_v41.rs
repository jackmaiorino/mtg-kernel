//! Complete ordered V40 authority plus current Herald facts, kept backend-only.
use super::*;
use crate::game_outcome_observation_v41::GameOutcomeObservationV41;

// No Debug or serialization: this contains the complete executable authority.
#[derive(Clone, PartialEq)]
pub(crate) struct GameOutcomeDecisionBindingV41 {
    pub(crate) predecessor: akroma_v40::AkromaDecisionBindingV40,
    observation: GameOutcomeObservationV41,
}

impl FastActorSessionV1 {
    pub(crate) fn into_game_outcome_v41(self) -> Result<Self, RlContractError> {
        let session = self.into_akroma_v40()?;
        let selector = crate::rules_meaning_game_outcome_protection_v1::live_selector()
            .map_err(|_| RlContractError("Herald selector unavailable".into()))?;
        crate::game_outcome_protection_public_v1::capture(&session.state, &selector)
            .map_err(|_| RlContractError("Herald current state invalid".into()))?;
        Ok(session)
    }

    pub(crate) fn game_outcome_restore_v41(
        &mut self,
        snapshot: &FastActorSessionSnapshotV1,
    ) -> Result<(), RlContractError> {
        let selector = crate::rules_meaning_game_outcome_protection_v1::live_selector()
            .map_err(|_| RlContractError("Herald selector unavailable".into()))?;
        crate::game_outcome_protection_public_v1::capture(&snapshot.0.state, &selector)
            .map_err(|_| RlContractError("Herald restored state invalid".into()))?;
        self.akroma_restore_v40(snapshot)
    }
    pub(crate) fn game_outcome_current_decision_input_v41(
        &self,
        expected: FastActorDecisionV1,
        seat: PlayerSeatV1,
    ) -> Result<
        (
            GameOutcomeObservationV41,
            Vec<ActionSemanticV1>,
            GameOutcomeDecisionBindingV41,
        ),
        FlatActionDecisionSliceErrorV1,
    > {
        let (old, actions, predecessor) = self.akroma_current_decision_input_v40(expected, seat)?;
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        let observation = crate::game_outcome_observation_v41::observe_v41(
            &self.state,
            &self.surface,
            current.actor,
            self.policy_step_count,
            current.physical_decision_id,
            (current.substep_index, current.substep_count),
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        if observation.predecessor != old {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        let binding = GameOutcomeDecisionBindingV41 {
            predecessor,
            observation: observation.clone(),
        };
        Ok((observation, actions, binding))
    }

    pub(crate) fn game_outcome_validate_bound_decision_v41(
        &self,
        expected: FastActorDecisionV1,
        seat: PlayerSeatV1,
        binding: &GameOutcomeDecisionBindingV41,
    ) -> Result<(), FlatActionDecisionSliceErrorV1> {
        let (_, _, current) = self.game_outcome_current_decision_input_v41(expected, seat)?;
        if &current != binding {
            return Err(FlatActionDecisionSliceErrorV1::CorruptCurrentBinding);
        }
        Ok(())
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

    #[test]
    #[cfg(all(
        feature = "limited-fdn-fixtures",
        not(feature = "standard-magezero-fixtures")
    ))]
    fn paid_herald_common_facts_binding_and_legacy_refusal_before_work_both_seats() {
        for actor in [PlayerId::P0, PlayerId::P1] {
            let (state, _) = crate::game_outcome_protection_v1::tests::cast_herald(actor);
            let before = serde_json::to_vec(&state).unwrap();
            let surface = PolicySurfaceV5::new_for_session();
            assert!(crate::rl::observe_v1(&state, actor, 0).is_err());
            assert!(crate::rl::observe_policy_v8(&state, &surface, actor, 0, 0, 0, 1).is_err());
            assert_eq!(serde_json::to_vec(&state).unwrap(), before);
            let legacy = FastActorSessionV1::from_human_v8_fixture_state(state.clone());
            assert!(matches!(
                legacy.current_response(),
                FastActorResponseV1::Terminal(_)
            ));
            assert_eq!(serde_json::to_vec(&legacy.state).unwrap(), before);

            let session = FastActorSessionV1::from_akroma_v40_fixture_state(state);
            let FastActorResponseV1::Decision(expected) = session.current_response() else {
                panic!("genuine common successor priority")
            };
            let seat = expected.acting_player;
            let (observation, _, binding) = session
                .game_outcome_current_decision_input_v41(expected, seat)
                .unwrap();
            assert!(observation.protection.cannot_lose[actor.index()]);
            let mut forged = binding.clone();
            forged.observation.protection.cannot_lose[actor.index()] = false;
            let unchanged = session.privileged_core_environment_hash();
            assert!(session
                .game_outcome_validate_bound_decision_v41(expected, seat, &forged)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), unchanged);
            let mut forged = binding;
            let common = forged
                .predecessor
                .families
                .iter_mut()
                .find_map(|family| match family {
                    crate::akroma_family_public_v40::FamilyProjectionV40::GameOutcome(facts) => {
                        Some(facts)
                    }
                    _ => None,
                })
                .unwrap();
            common.facts.protection.cannot_win[actor.opponent().index()] = false;
            assert!(session
                .game_outcome_validate_bound_decision_v41(expected, seat, &forged)
                .is_err());
            assert_eq!(session.privileged_core_environment_hash(), unchanged);
        }
    }
}
