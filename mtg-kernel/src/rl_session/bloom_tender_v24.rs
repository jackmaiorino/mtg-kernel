//! A read-only supplement to the existing authenticated V8 backend input.
use super::*;
type BloomTenderInputV24 = (
    crate::bloom_tender_public_v1::BloomTenderObservationV1,
    Vec<ActionSemanticV1>,
    human_v8::HumanDecisionBindingV8,
);
impl FastActorSessionV1 {
    pub(crate) fn bloom_tender_current_input_v24(
        &self,
        expected: FastActorDecisionV1,
        seat: PlayerSeatV1,
    ) -> Result<BloomTenderInputV24, FlatActionDecisionSliceErrorV1> {
        let (observation, actions, binding) =
            self.human_current_decision_input_v8(expected, seat)?;
        let public = crate::bloom_tender_public_v1::from_input(&self.state, observation, &actions)
            .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)?;
        Ok((public, actions, binding))
    }
}
