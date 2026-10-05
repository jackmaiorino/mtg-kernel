use super::{FrozenPlayDecisionScoresV1, FrozenPlayPolicyV1};
use crate::gameplay_trace_v1::*;
use crate::rl_session::{FastActorDecisionV1, FastActorSessionV1};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
#[cfg(test)]
mod tests;

impl FrozenPlayPolicyV1 {
    pub fn enable_gameplay_trace_v1(
        &mut self,
        config: GameplayTraceConfigV1,
    ) -> Result<(), String> {
        if self.trace.is_some() || self.collection_sampler.is_some() {
            return Err(
                "trace requires a frozen quantized gameplay sampler and one sidecar".into(),
            );
        }
        let identity = json!({"actual_model":self.actual_model_identity_v1(),"origin":self.identity_v1(),
            "runtime":runtime_identity()?,"sampler":self.runtime_sampler_identity_v1(),
            "sampler_max_actions":self.runtime_sampler_max_actions_v1(),
            "sampling_settings":{"temperature":1,"q_bits":8,"gap_clamp_nats":16,"total_mass":"18446744073709551616",
                "narrow_contract":crate::fast_sampler::FAST_CATEGORICAL_SAMPLER_CONTRACT_JSON,
                "wide_contract":crate::fast_sampler::WIDE_CATEGORICAL_SAMPLER_CONTRACT_JSON_V1}});
        self.trace = Some(GameplayTraceV1::open(config, identity)?);
        Ok(())
    }

    pub(crate) fn capture_trace_v1(
        &self,
        session: &FastActorSessionV1,
        decision: FastActorDecisionV1,
        selected: u32,
        scores: &FrozenPlayDecisionScoresV1,
    ) {
        let Some(handle) = &self.trace else {
            return;
        };
        if !GameplayTraceV1::accepting(handle) {
            return;
        }
        let result = (|| -> Result<Value, String> {
            let (observation, actions) =
                session.gameplay_trace_visible_v1(decision, self.fresh_successor.is_some())?;
            let labels = crate::human_bo3_v1::gameplay_trace_labels_v1(&observation, &actions)
                .map_err(|_| "unsupported actor-visible label")?;
            if actions.len() != scores.logits.len() {
                return Err("trace menu count differs".into());
            }
            let width = scores.logits.len();
            let (tensor, masses, generation) = if let Some(fresh) = &self.fresh_successor {
                (
                    &fresh.tensor.common,
                    fresh.sampler.last_masses_v1(width),
                    "V4",
                )
            } else if let Some(successor) = &self.successor {
                (
                    &successor.tensor.common,
                    successor.sampler.last_masses_v1(width),
                    "V3",
                )
            } else {
                (&self.tensor, self.sampler.last_masses_v1(width), "V2")
            };
            let behavior = crate::phase1_agent_v1::BehaviorDistributionV1::HamiltonQ64 {
                selected_index: selected,
                mass_numerators: masses.iter().map(u128::to_string).collect(),
            };
            behavior.selected_probability_v1(width)?;
            let ordered = actions.iter().zip(labels).enumerate().map(|(index,(semantic,label))|
                json!({"engine_index":index,"label":label,"semantic":semantic})).collect::<Vec<_>>();
            let tensor = tensor_value(tensor);
            Ok(json!({"schema":SCHEMA_V1,"kind":"decision","decision":{
                "episode_id":decision.episode_id,"step":decision.step,
                "physical_decision_id":decision.physical_decision_id,
                "substep_index":decision.substep_index,"substep_count":decision.substep_count,
                "legal_action_count":decision.legal_action_count},
                "actor":decision.acting_player,"feature_generation":generation,
                "observation":observation,"ordered_actions":ordered,"encoded_input":tensor,
                "encoded_input_sha256":format!("{:x}",Sha256::digest(serde_json::to_vec(&tensor).map_err(|e|e.to_string())?)),
                "logits":scores.logits,"logit_bits":scores.logits.iter().map(|v|v.to_bits()).collect::<Vec<_>>(),
                "value":scores.value,"value_bits":scores.value.to_bits(),"behavior":behavior,
                "rng":self.trace_draw,"selected_index":selected}))
        })();
        match result {
            Ok(record) => session.arm_gameplay_trace_v1(PendingTrace {
                handle: handle.clone(),
                decision,
                selected,
                record,
            }),
            Err(_) => GameplayTraceV1::error(handle, "capture_failed_actor_visible_projection"),
        }
    }
}
