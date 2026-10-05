use super::*;
use crate::gameplay_trace_v1::{actor_id, GameplayTraceV1, PendingTrace, SCHEMA_V1};
use serde_json::json;

impl FastActorSessionV1 {
    pub(crate) fn gameplay_trace_visible_v1(
        &self,
        expected: FastActorDecisionV1,
        v4: bool,
    ) -> Result<
        (
            crate::policy_observation_v6::ObservationV6,
            Vec<ActionSemanticV1>,
        ),
        String,
    > {
        if self.current_response() != FastActorResponseV1::Decision(expected) {
            return Err("trace decision binding".into());
        }
        let observation = if v4 {
            self.flat_policy_observation_v4(expected)
        } else {
            crate::rl::observe_policy_v6_unhashed_for_flat_policy(
                &self.state,
                &self.surface,
                actor_id(expected.acting_player),
                expected.step,
                expected.physical_decision_id,
                expected.substep_index,
                expected.substep_count,
            )
            .map_err(|_| FlatActionDecisionSliceErrorV1::CorruptCurrentBinding)
        }
        .map_err(|_| "trace decision projection")?;
        let current = self.current.as_ref().ok_or("trace has no decision")?;
        Ok((
            observation,
            current
                .candidates
                .iter()
                .map(|c| c.semantic.clone())
                .collect(),
        ))
    }

    pub(crate) fn arm_gameplay_trace_v1(&self, mut pending: PendingTrace) {
        let Some(current) = self.current.as_ref() else {
            return;
        };
        if self.current_response() != FastActorResponseV1::Decision(pending.decision) {
            GameplayTraceV1::error(&pending.handle, "stale_decision_capture");
            return;
        }
        let candidate = &current.candidates[pending.selected as usize];
        let kind = match &candidate.policy_action {
            PolicyActionV5::Surface(_) => "surface",
            PolicyActionV5::ChooseAttackerInclusion { .. } => "attacker_inclusion",
            PolicyActionV5::ChooseBlockerInclusion { .. } => "blocker_inclusion",
        };
        pending.record["bound_engine_action"] = json!({"policy_action_kind":kind,
            "semantic":candidate.semantic,"engine_index":pending.selected,
            "environment_revision":current.environment_revision,
            "episode_id":self.episode_id,"step":self.policy_step_count,
            "binding":"actual current.candidates[index] consumed by FastActorCurrentCandidateProofV1"});
        let Ok(mut slot) = self.trace_pending.0.lock() else {
            return;
        };
        if let Some(previous) = slot.take() {
            GameplayTraceV1::error(&previous.handle, "sampled_decision_not_executed");
        }
        *slot = Some(pending);
    }

    pub(crate) fn finish_gameplay_trace_v1(
        &self,
        mut pending: PendingTrace,
        episode: u64,
        step: u64,
        selected: u32,
        result: &Result<FastActorResponseV1, RlSessionError>,
    ) {
        if (episode, step, selected)
            != (
                pending.decision.episode_id,
                pending.decision.step,
                pending.selected,
            )
        {
            GameplayTraceV1::error(&pending.handle, "executed_binding_differs");
            return;
        }
        // Fixed recorded actor even when the next decision belongs to the opponent.
        let observation = crate::rl::observe_policy_v6_unhashed_for_flat_policy(
            &self.state,
            &self.surface,
            actor_id(pending.decision.acting_player),
            self.policy_step_count,
            self.physical_decision_count,
            0,
            1,
        );
        let Ok(observation) = observation else {
            GameplayTraceV1::error(&pending.handle, "actor_transition_unavailable");
            return;
        };
        let next = match self.current_response() {
            FastActorResponseV1::Decision(d) => json!({"kind":"decision","episode_id":d.episode_id,
                "step":d.step,"actor":d.acting_player}),
            FastActorResponseV1::Terminal(t) => {
                json!({"kind":"terminal","classification":t.terminal_classification,
                "outcome":t.terminal_outcome})
            }
        };
        pending.record["transition"] = json!({"schema":SCHEMA_V1,"applied":result.is_ok(),
            "actor":pending.decision.acting_player,"observation":observation,"next":next});
        GameplayTraceV1::record(&pending.handle, pending.record);
    }
}
