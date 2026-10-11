//! Backend-only public Raid input for the sealed successor session.
use super::*;

impl FastActorSessionV1 {
    pub(crate) fn flat_raid_observation_v9(
        &self,
        expected: FastActorDecisionV1,
    ) -> Result<crate::policy_observation_v9::ObservationV9, FlatActionDecisionSliceErrorV1> {
        // Validate the complete inherited action authority before reading
        // declaration history. The owning successor never exposes this session.
        self.flat_policy_observation_v5(expected)?;
        let current = self
            .current
            .as_ref()
            .ok_or(FlatActionDecisionSliceErrorV1::NoCurrentDecision)?;
        crate::policy_observation_v9::observe_policy_v9(
            &self.state,
            &self.surface,
            current.actor,
            self.policy_step_count,
            current.physical_decision_id,
            current.substep_index,
            current.substep_count,
        )
        .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)
    }
}
