//! The `learner-checkpoint:<path>` evaluation policy of
//! `docs/design/limited_campaign_v1.md` Section 4.2.
//!
//! A checkpoint is a `limited-campaign-checkpoint/v1` file
//! (`limited_campaign_v1::checkpoint`). It is read once, when its spec is
//! parsed: the reader refuses non-canonical bytes, a malformed record and a
//! feature identity other than this build's, the learner is restored through
//! its own `restore`, and the run is pinned by the SHA-256 of the file bytes.
//! Each decision with more than one legal action scores the actor's
//! `KaitoSession::training_input_v1` with the checkpoint's model and samples
//! one action from the model's `f64` logits with the campaign's sampler
//! `limited_campaign_v1::sample_logits_v1` (`f64-exp-icdf-u53/v1`, weights
//! from the libm-free `exp_v1`), the same function the training runner
//! samples with. A forced decision (one legal action) takes index 0 without a
//! model call or a draw.
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
    LearnerV2, LearnerV2H16, SnapshotV2, CONTRACT_V2, CONTRACT_V2_H16,
};
use crate::limited_campaign_v1::checkpoint::{CheckpointPolicyV1, CheckpointV1};
use std::path::Path;
use std::sync::Arc;

/// Domain tag of a learner seat's evaluation stream: the ASCII text
/// `mtg-kernel/limited-campaign-eval-learner-rng/v1` and one NUL byte.
pub const LEARNER_STREAM_TAG_V1: &[u8] = b"mtg-kernel/limited-campaign-eval-learner-rng/v1\0";
/// The contract written to every match summary that seats a learner.
pub const LEARNER_POLICY_CONTRACT_V1: &str = "learner-checkpoint/v1: \
    limited-campaign-checkpoint/v1 model forward on KaitoSession::training_input_v1; \
    sampler limited_campaign_v1::sample_logits_v1 (f64-exp-icdf-u53/v1, exp_v1 \
    weights, gap >= -708, else 0), one next_u64 per decision with more than one \
    legal action; a forced decision takes index 0 without a draw; seat stream \
    SplitMix64(derive_seed(tag, [seed, pair, leg, seat])) with tag \
    mtg-kernel/limited-campaign-eval-learner-rng/v1 and one NUL byte";

/// The seed of the learner stream of seat `seat` in game `stream`.
pub fn learner_stream_seed_v1(stream: MatchStreamV1, seat: u64) -> u64 {
    derive_seed(
        LEARNER_STREAM_TAG_V1,
        &[stream.seed, stream.pair, stream.leg, seat],
    )
}

/// One action of the logits' legal menu for the 64-bit draw `draw`: the
/// campaign sampler `limited_campaign_v1::sample_logits_v1`.
pub fn sample_learner_action_v1(logits: &[f64], draw: u64) -> Result<usize, String> {
    crate::limited_campaign_v1::sample_logits_v1(logits, draw)
}

/// A loaded, verified learner checkpoint. Two checkpoints are equal when
/// their paths and file digests are.
#[derive(Clone)]
pub struct LearnerCheckpointV1 {
    path: String,
    sha256: String,
    step: u64,
    contract: String,
    policy: Arc<CheckpointPolicyV1>,
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
    /// Reads and verifies the `limited-campaign-checkpoint/v1` file at `path`
    /// and restores its model.
    pub fn load_v1(path: &str) -> Result<Self, String> {
        let (checkpoint, sha256) = CheckpointV1::load_v1(Path::new(path))?;
        let policy = checkpoint
            .policy_v1()
            .map_err(|error| format!("{path}: {error}"))?;
        Ok(Self {
            path: path.into(),
            sha256,
            step: checkpoint.step,
            contract: checkpoint.learner_contract,
            policy: Arc::new(policy),
        })
    }

    /// A checkpoint player from a bare v2 learner snapshot, for tests and
    /// tools that hold no checkpoint file. `LearnerV2::restore` refuses
    /// another contract, another feature identity (which covers this build's
    /// learner and session sources), a bad configuration and malformed
    /// arrays.
    pub fn from_snapshot_v1(
        path: impl Into<String>,
        sha256: impl Into<String>,
        snapshot: SnapshotV2,
    ) -> Result<Self, String> {
        let path = path.into();
        let step = snapshot.step();
        let contract = snapshot.contract().to_string();
        let refuse = |error: crate::current_mixed_policy_v1::Error| format!("{path}: {error}");
        let policy = match contract.as_str() {
            CONTRACT_V2 => {
                let learner = LearnerV2::restore(snapshot).map_err(refuse)?;
                CheckpointPolicyV1::ActorCriticH32(learner.model().clone())
            }
            CONTRACT_V2_H16 => {
                let learner = LearnerV2H16::restore(snapshot).map_err(refuse)?;
                CheckpointPolicyV1::ActorCriticH16(learner.model().clone())
            }
            other => return Err(format!("{path}: unknown learner contract {other:?}")),
        };
        Ok(Self {
            path,
            sha256: sha256.into(),
            step,
            contract,
            policy: Arc::new(policy),
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
    pub fn contract(&self) -> &str {
        &self.contract
    }

    /// The model's logits over the input's legal menu, in menu order.
    pub fn logits_v1(&self, input: &AuthenticatedInputV1) -> Result<Vec<f64>, String> {
        self.policy.logits(input).map_err(|e| e.to_string())
    }

    /// One sampled action for `input` and the seat stream's next draw.
    pub fn sample_v1(&self, input: &AuthenticatedInputV1, draw: u64) -> Result<usize, String> {
        sample_learner_action_v1(&self.logits_v1(input)?, draw)
    }
}

#[cfg(test)]
#[path = "learner_policy_v1_tests.rs"]
mod tests;
