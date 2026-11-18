//! The `learner-checkpoint:<path>` evaluation policy of
//! `docs/design/limited_campaign_v1.md` Section 4.2.
//!
//! A checkpoint is read once, when its spec is parsed, and pinned by the
//! SHA-256 of its file bytes. Each decision with more than one legal action
//! scores the actor's `KaitoSession::training_input_v1` with the checkpoint's
//! `current_tree_learner_v2` model and samples one action from the model's
//! `f64` logits under the design's sampler `f64-exp-icdf-u53/v1` with the
//! libm-free weight of the research lead's ruling 11: binary64 gaps from the first
//! maximal logit, weight `unclamped_softmax_sampler_v1::exp_v1(gap)` (no
//! platform math library) for a gap at or above `-708` and `+0.0` below,
//! sequential prefix sums in legal-action order, and the smallest index whose
//! prefix sum exceeds `u * S` for the 53-bit uniform `u` of one `next_u64`.
//! A forced decision (one legal action) takes index 0 without a model call
//! or a draw.
//!
//! The draws come from the learner seat's own `SplitMix64` stream (the
//! research lead's ruling 5 on design Section 2.2), seeded with
//! `derive_seed(LEARNER_STREAM_TAG_V1, [match seed, pair, leg, seat])`, the
//! way the campaign's training streams are derived. Its samples therefore do
//! not depend on the opponent's draws, and the policies that share the
//! match's generator draw from it exactly as before.
use super::{derive_seed, MatchStreamV1};
use crate::current_tree_learner_v1::AuthenticatedInputV1;
use crate::current_tree_learner_v2::{
    LearnerV2, LearnerV2H16, ModelV2, ModelV2H16, SnapshotV2, CONTRACT_V2, CONTRACT_V2_H16,
};
use crate::unclamped_softmax_sampler_v1::{
    exp_v1, unclamped_softmax_select_v1, unclamped_softmax_unit_v1, UNCLAMPED_SOFTMAX_GAP_FLOOR_V1,
    UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1,
};
use std::sync::Arc;

/// Domain tag of a learner seat's evaluation stream: the ASCII text
/// `mtg-kernel/limited-campaign-eval-learner-rng/v1` and one NUL byte.
pub const LEARNER_STREAM_TAG_V1: &[u8] = b"mtg-kernel/limited-campaign-eval-learner-rng/v1\0";
/// The contract written to every match summary that seats a learner.
pub const LEARNER_POLICY_CONTRACT_V1: &str = "learner-checkpoint/v1: \
    current_tree_learner_v2 forward on KaitoSession::training_input_v1; \
    sampler f64-exp-icdf-u53/v1 with unclamped_softmax_sampler_v1::exp_v1 \
    weights (gap >= -708, else 0) and one next_u64 per decision with more \
    than one legal action; a forced decision takes index 0 without a draw; \
    seat stream SplitMix64(derive_seed(tag, [seed, pair, leg, seat])) with \
    tag mtg-kernel/limited-campaign-eval-learner-rng/v1 and one NUL byte";

/// The seed of the learner stream of seat `seat` in game `stream`.
pub fn learner_stream_seed_v1(stream: MatchStreamV1, seat: u64) -> u64 {
    derive_seed(
        LEARNER_STREAM_TAG_V1,
        &[stream.seed, stream.pair, stream.leg, seat],
    )
}

/// One action of the logits' legal menu for the 64-bit draw `draw` (see the
/// module documentation). Fails closed on an empty or oversized menu, a
/// non-finite logit or an unpinned floating-point environment.
pub fn sample_learner_action_v1(logits: &[f64], draw: u64) -> Result<usize, String> {
    crate::deterministic_math_v1::verify_pinned_mxcsr_state_v1().map_err(|error| {
        format!(
            "learner sampling requires pinned MXCSR (found {:#010x})",
            error.observed_v1()
        )
    })?;
    if logits.is_empty() || logits.len() > UNCLAMPED_SOFTMAX_MAX_ACTIONS_V1 {
        return Err(format!("learner sampling over {} actions", logits.len()));
    }
    if let Some(index) = logits.iter().position(|logit| !logit.is_finite()) {
        return Err(format!("learner logit {index} is not finite"));
    }
    let mut maximum = logits[0];
    for &logit in logits {
        if logit > maximum {
            maximum = logit;
        }
    }
    let mut cumulative = 0.0_f64;
    let prefix: Vec<f64> = logits
        .iter()
        .map(|&logit| {
            let gap = logit - maximum;
            cumulative += if gap >= UNCLAMPED_SOFTMAX_GAP_FLOOR_V1 {
                exp_v1(gap)
            } else {
                0.0
            };
            cumulative
        })
        .collect();
    unclamped_softmax_select_v1(&prefix, unclamped_softmax_unit_v1(draw))
        .map_err(|error| error.to_string())
}

#[derive(Clone)]
enum LearnerModelV1 {
    Wide(Arc<ModelV2>),
    Narrow(Arc<ModelV2H16>),
}

/// A loaded, verified learner checkpoint. Two checkpoints are equal when
/// their paths and file digests are.
#[derive(Clone)]
pub struct LearnerCheckpointV1 {
    path: String,
    sha256: String,
    step: u64,
    contract: &'static str,
    model: LearnerModelV1,
}

impl PartialEq for LearnerCheckpointV1 {
    fn eq(&self, other: &Self) -> bool {
        self.path == other.path && self.sha256 == other.sha256
    }
}
impl Eq for LearnerCheckpointV1 {}

impl std::fmt::Debug for LearnerCheckpointV1 {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LearnerCheckpointV1")
            .field("path", &self.path)
            .field("sha256", &self.sha256)
            .field("step", &self.step)
            .field("contract", &self.contract)
            .finish()
    }
}

impl LearnerCheckpointV1 {
    /// Reads and verifies the checkpoint file at `path`.
    pub fn load_v1(path: &str) -> Result<Self, String> {
        Err(format!(
            "learner-checkpoint:{path}: reading limited-campaign-checkpoint/v1 files is not built yet"
        ))
    }

    /// A checkpoint from its learner snapshot. `LearnerV2::restore` refuses
    /// another contract, another feature identity (which covers this build's
    /// learner and session sources), a bad configuration and malformed
    /// arrays, so a checkpoint from another build does not load.
    pub fn from_snapshot_v1(
        path: impl Into<String>,
        sha256: impl Into<String>,
        snapshot: SnapshotV2,
    ) -> Result<Self, String> {
        let path = path.into();
        let step = snapshot.step();
        let named = snapshot.contract().to_string();
        let (contract, model) = match named.as_str() {
            CONTRACT_V2 => (
                CONTRACT_V2,
                LearnerModelV1::Wide(Arc::new(
                    LearnerV2::restore(snapshot)
                        .map_err(|error| format!("{path}: {error}"))?
                        .model()
                        .clone(),
                )),
            ),
            CONTRACT_V2_H16 => (
                CONTRACT_V2_H16,
                LearnerModelV1::Narrow(Arc::new(
                    LearnerV2H16::restore(snapshot)
                        .map_err(|error| format!("{path}: {error}"))?
                        .model()
                        .clone(),
                )),
            ),
            other => return Err(format!("{path}: unknown learner contract {other:?}")),
        };
        Ok(Self {
            path,
            sha256: sha256.into(),
            step,
            contract,
            model,
        })
    }

    pub fn path(&self) -> &str {
        &self.path
    }
    /// SHA-256 of the checkpoint file bytes.
    pub fn sha256(&self) -> &str {
        &self.sha256
    }
    pub fn step(&self) -> u64 {
        self.step
    }
    pub fn contract(&self) -> &'static str {
        self.contract
    }

    /// The model's logits over the input's legal menu, in menu order.
    pub fn logits_v1(&self, input: &AuthenticatedInputV1) -> Result<Vec<f64>, String> {
        let output = match &self.model {
            LearnerModelV1::Wide(model) => model.forward(input),
            LearnerModelV1::Narrow(model) => model.forward(input),
        }
        .map_err(|error| error.to_string())?;
        Ok(output.logits)
    }

    /// One sampled action for `input` and the seat stream's next draw.
    pub fn sample_v1(&self, input: &AuthenticatedInputV1, draw: u64) -> Result<usize, String> {
        sample_learner_action_v1(&self.logits_v1(input)?, draw)
    }
}

#[cfg(test)]
#[path = "learner_policy_v1_tests.rs"]
mod tests;
