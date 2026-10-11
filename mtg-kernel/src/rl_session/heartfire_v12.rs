//! Private validated input for the sealed Heartfire consumer.
use super::*;
impl FastActorSessionV1 {
    pub(crate) fn human_heartfire_public_v12(
        &self,
        expected: FastActorDecisionV1,
        human: PlayerSeatV1,
    ) -> Result<
        crate::heartfire_public_v1::HeartfirePublicObservationV1,
        FlatActionDecisionSliceErrorV1,
    > {
        let base = self.human_raid_observation_v9(expected, human)?;
        crate::heartfire_public_v1::from_authenticated_v9(&self.state, base)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)
    }
}
